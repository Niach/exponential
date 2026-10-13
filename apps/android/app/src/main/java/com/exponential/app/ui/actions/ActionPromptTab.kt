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
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

// The action editor (EXP-694; EXP-1291/EXP-1296: an action is a SAVED
// PROMPT, so its page IS this editor): icon + name, the composer hint, the
// typed pick inputs (repo | board | pr | issue) and the markdown PROMPT, on
// the grouped card stack every form uses. [ActionPromptForm] is the one form
// — the page edits a fetched row with it, `New action` creates with it.
//
// The synced `actions` shape excludes the ≤64KB body, so the page fetches
// the row when it opens (`actions.get`) and writes it back with
// `actions.update`. Writes are owner-gated server-side; a member gets the
// same fields read-only, no Save, no Delete.

// Mirrors the router's zod limits (apps/web/src/lib/trpc/actions.ts) so a
// too-long value is impossible rather than a round-trip refusal.
private const val MAX_ACTION_NAME = 255
private const val MAX_ACTION_BODY = 64 * 1024
private const val MAX_ACTION_INPUTS = 10
private const val MAX_ACTION_INPUT_LABEL = 100

/** How each contract `actionInputType` reads in the editor. */
internal fun actionInputTypeLabel(type: String): String = when (type) {
    "repo" -> "Repository"
    "board" -> "Board"
    "pr" -> "Pull request"
    "issue" -> "Issue"
    else -> type
}

/** The editor's field state (one per page / create sheet). */
@Stable
class ActionFormState {
    var name by mutableStateOf("")
    var icon by mutableStateOf("")
    var body by mutableStateOf("")
    var promptPlaceholder by mutableStateOf("")
    var inputs by mutableStateOf<List<ActionInputDto>>(emptyList())

    fun seed(row: ActionDto) {
        name = row.name
        icon = row.icon.orEmpty()
        body = row.body
        promptPlaceholder = row.promptPlaceholder.orEmpty()
        inputs = row.inputs.orEmpty()
    }

    fun fields(): ActionFields = ActionFields(
        name = name.trim(),
        icon = icon.takeIf { it.isNotEmpty() },
        body = body,
        inputs = inputs.map { it.copy(label = it.label.trim()) },
        promptPlaceholder = promptPlaceholder.trim().takeIf { it.isNotEmpty() },
    )

    fun differsFrom(row: ActionDto): Boolean =
        name.trim() != row.name ||
            icon != row.icon.orEmpty() ||
            body != row.body ||
            promptPlaceholder.trim() != row.promptPlaceholder.orEmpty() ||
            inputs.map { it.copy(label = it.label.trim()) } != row.inputs.orEmpty()

    val valid: Boolean
        get() = name.isNotBlank() && body.isNotBlank() && inputs.all { it.label.isNotBlank() }

    fun addInput(type: String = "issue") {
        if (inputs.size >= MAX_ACTION_INPUTS) return
        val label = actionInputTypeLabel(type)
        inputs = inputs + ActionInputDto(
            key = actionInputKey(label, inputs.map { it.key }.toSet()),
            label = label,
            type = type,
        )
    }

    fun updateInput(index: Int, transform: (ActionInputDto) -> ActionInputDto) {
        inputs = inputs.mapIndexed { i, def -> if (i == index) transform(def) else def }
    }

    fun removeInput(index: Int) {
        inputs = inputs.filterIndexed { i, _ -> i != index }
    }
}

@Composable
fun rememberActionFormState(): ActionFormState = remember { ActionFormState() }

/**
 * THE action form: identity row, composer hint, inputs, prompt. [bodyLoading]
 * parks the prompt (the `actions.get` fetch); [bodySlot] replaces it (the
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

        // EXP-825: what the requester should type beside this action — the
        // composer field's placeholder while it is picked. Blank clears.
        OptionGroup {
            GlassTextField(
                value = form.promptPlaceholder,
                onValueChange = { form.promptPlaceholder = it.take(ActionDto.PROMPT_PLACEHOLDER_MAX_LENGTH) },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("action-edit-prompt-placeholder"),
                placeholder = "Composer hint, e.g. Scope: which platforms, which version",
                singleLine = true,
                enabled = editable,
                bordered = false,
            )
        }
        Spacer(Modifier.height(16.dp))

        // The typed pick inputs the composer asks for when the action runs.
        SectionHeader(
            "Inputs",
            modifier = Modifier.padding(horizontal = 16.dp),
        ) {
            if (editable && form.inputs.size < MAX_ACTION_INPUTS) {
                GlassPill(
                    "Add input",
                    icon = ExpIcons.uiAdd,
                    size = PillSize.Sm,
                    onClick = { form.addInput() },
                    modifier = Modifier.testTag("action-edit-add-input"),
                )
            }
        }
        form.inputs.forEachIndexed { index, def ->
            OptionGroup {
                TextFieldRow(
                    label = "Label",
                    value = def.label,
                    onValueChange = { value ->
                        form.updateInput(index) { it.copy(label = value.take(MAX_ACTION_INPUT_LABEL)) }
                    },
                    enabled = editable,
                    trailing = if (editable) {
                        {
                            CircleIconButton(
                                ExpIcons.uiDelete,
                                contentDescription = "Remove input",
                                onClick = { form.removeInput(index) },
                                tint = DesignTokens.Semantic.Red.copy(alpha = 0.5f),
                                glyphSize = 16.dp,
                                borderless = true,
                            )
                        }
                    } else {
                        null
                    },
                )
                GroupDivider()
                PickerRow(
                    label = "Type",
                    value = actionInputTypeLabel(def.type),
                    options = DomainContract.actionInputTypeValues,
                    selected = def.type,
                    optionLabel = ::actionInputTypeLabel,
                    onSelect = { type -> form.updateInput(index) { it.copy(type = type) } },
                    enabled = editable,
                )
                GroupDivider()
                SwitchRow(
                    title = "Required",
                    checked = def.required,
                    onCheckedChange = { required -> form.updateInput(index) { it.copy(required = required) } },
                    enabled = editable,
                )
            }
            Spacer(Modifier.height(8.dp))
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
