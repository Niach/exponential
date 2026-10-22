package com.exponential.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.api.PromptDto
import com.exponential.app.data.api.PromptFields
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// NEW-2/NEW-7: Settings → Prompts, built like Settings → Agents (web
// `components/team/prompts-section.tsx` + `components/prompt-form.tsx`): the
// list = the "Prompts" band (`New prompt` as its owner-only trailing pill)
// over one flat row per prompt (icon + name); a row opens the prompt's page
// (THE form: icon + name, the markdown body, Save changes, Delete prompt for
// owners); `New prompt` opens the same page in create mode. Members read
// every page; the owner-only controls are HIDDEN from them. A prompt RUNS
// from the composer's `/` menu only.

internal const val PROMPTS_TITLE = "Prompts"
internal const val NEW_PROMPT_LABEL = "New prompt"
private const val CREATE_PROMPT_LABEL = "Create prompt"
private const val SAVE_LABEL = "Save changes"
private const val DELETE_PROMPT_LABEL = "Delete prompt"
private const val NO_PROMPTS = "No prompts yet"

// The router's zod limits (apps/web/src/lib/trpc/prompts.ts): over them is
// an input error under the field, never a round-trip refusal.
private const val MAX_PROMPT_NAME = 255
internal const val PROMPT_BODY_MAX_CHARS = 64 * 1024

/** The Prompts band, embeddable (Team settings' section; the page below). */
@Composable
fun PromptsSettingsList(
    onOpenPrompt: (promptId: String) -> Unit,
    onNewPrompt: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: PromptSettingsViewModel = hiltViewModel(key = "prompts-list"),
) {
    val prompts by viewModel.list.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    PromptsSettingsListContent(
        prompts = prompts,
        canCreate = isOwner,
        onOpenPrompt = onOpenPrompt,
        onNewPrompt = onNewPrompt,
        modifier = modifier,
    )
}

/** The stateless band: null [prompts] = still loading (skeleton rows). */
@Composable
internal fun PromptsSettingsListContent(
    prompts: List<PromptDto>?,
    canCreate: Boolean,
    onOpenPrompt: (promptId: String) -> Unit,
    onNewPrompt: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.testTag("prompts-settings-list")) {
        SectionHeader(PROMPTS_TITLE) {
            if (canCreate) {
                GlassPill(
                    NEW_PROMPT_LABEL,
                    icon = ExpIcons.uiAdd,
                    size = PillSize.Sm,
                    onClick = onNewPrompt,
                    modifier = Modifier.testTag("prompts-new"),
                )
            }
        }
        when {
            prompts == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
            prompts.isEmpty() -> EmptyState(
                title = NO_PROMPTS,
                icon = ExpIcons.navActions,
                size = EmptyStateSize.List,
            )
            else -> prompts.forEach { prompt ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("prompts-list-row-${prompt.id}")
                        .flatRow()
                        .clickable { onOpenPrompt(prompt.id) }
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                ) {
                    Icon(
                        actionGlyph(prompt),
                        contentDescription = null,
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Text(
                        prompt.name,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

/** Settings → Prompts as its own pushed page (`settings/prompts`). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PromptsSettingsScreen(
    onBack: () -> Unit,
    onOpenPrompt: (promptId: String) -> Unit,
    onNewPrompt: () -> Unit,
) {
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(PROMPTS_TITLE) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        PromptsSettingsList(
            onOpenPrompt = onOpenPrompt,
            onNewPrompt = onNewPrompt,
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
        )
    }
}

/** THE prompt form's field state (one per page). */
@Stable
internal class PromptFormState {
    var name by mutableStateOf("")
    var icon by mutableStateOf("")
    var body by mutableStateOf("")
    var touched by mutableStateOf(false)

    fun seed(row: PromptDto) {
        name = row.name
        icon = row.icon.orEmpty()
        body = row.body
    }

    fun fields(): PromptFields = PromptFields(name = name.trim(), icon = icon.takeIf { it.isNotEmpty() }, body = body)

    fun differsFrom(row: PromptDto?): Boolean =
        row == null || name.trim() != row.name || icon != row.icon.orEmpty() || body != row.body

    /** Input errors under their fields (FEED-79). */
    fun nameError(creating: Boolean): String? =
        if ((touched || !creating) && name.isBlank()) "Enter a name" else null

    val bodyError: String?
        get() = if (body.length > PROMPT_BODY_MAX_CHARS) {
            "Too long: ${body.length} of $PROMPT_BODY_MAX_CHARS characters"
        } else {
            null
        }

    val valid: Boolean get() = name.isNotBlank() && body.isNotBlank() && bodyError == null
}

/**
 * ONE prompt's page (`settings/prompt/{promptId}`), or `New prompt`
 * (`settings/prompt-new`, the model's `promptId` null). [onCreated] lands
 * on a just-created prompt's page; [onDeleted] leaves the page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PromptSettingsScreen(
    onBack: () -> Unit,
    onCreated: (promptId: String) -> Unit,
    onDeleted: () -> Unit,
    viewModel: PromptSettingsViewModel = hiltViewModel(),
) {
    val creating = viewModel.promptId == null
    val row by viewModel.row.collectAsStateWithLifecycle()
    val state by viewModel.state.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    // One fetch per page visit: a body a teammate changed is re-read.
    LaunchedEffect(viewModel.promptId) { viewModel.load() }
    val form = remember { PromptFormState() }
    // Seed ONCE per fetched row: a save rewrites `state.prompt`, and
    // re-seeding would fight a person who kept typing.
    var seededId by remember { mutableStateOf<String?>(null) }
    val loaded = state.prompt
    LaunchedEffect(loaded?.id) {
        val fetched = loaded ?: return@LaunchedEffect
        if (seededId == fetched.id) return@LaunchedEffect
        seededId = fetched.id
        form.seed(fetched)
    }
    var confirmDelete by remember { mutableStateOf(false) }

    val editable = isOwner && !state.saving && (creating || loaded != null)
    val canSubmit = editable && form.valid && form.differsFrom(loaded) && !state.loading && !state.loadFailed
    val title = if (creating) NEW_PROMPT_LABEL else row?.name ?: loaded?.name.orEmpty()

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .consumeWindowInsets(padding)
                .imePadding()
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag("prompt-page"),
        ) {
            PromptForm(
                form = form,
                editable = editable,
                creating = creating,
                bodyLoading = state.loading,
                bodyFailed = state.loadFailed,
                onRetry = { viewModel.load(retried = true) },
            )
            // A member reads the prompt: no Save, no Delete.
            if (isOwner) {
                Spacer(Modifier.height(12.dp))
                GlassSubmitButton(
                    label = if (creating) CREATE_PROMPT_LABEL else SAVE_LABEL,
                    enabled = canSubmit,
                    onClick = {
                        form.touched = true
                        if (!form.valid) return@GlassSubmitButton
                        if (creating) viewModel.create(form.fields(), onCreated) else viewModel.save(form.fields())
                    },
                    modifier = Modifier.testTag(if (creating) "prompt-create" else "prompt-save"),
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
                if (!creating && row != null) {
                    Spacer(Modifier.height(12.dp))
                    GlassPill(
                        DELETE_PROMPT_LABEL,
                        icon = ExpIcons.uiDelete,
                        onClick = { confirmDelete = true },
                        contentColor = DesignTokens.Semantic.Red,
                        modifier = Modifier.fillMaxWidth().testTag("prompt-delete"),
                    )
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }

    val deleting = row
    if (confirmDelete && deleting != null) {
        // Web parity: `Delete "<name>"?`, no body line (a prompt carries no
        // triggers; the sessions it started stay).
        val prompt = Prompts.DeleteAction.spec.render("title", null, "name" to deleting.name)
        PromptAlert(
            prompt = prompt,
            onDismiss = { confirmDelete = false },
            handlers = prompt.actions.filter { it.role != Prompts.Role.Cancel }.associate { action ->
                action.id to {
                    confirmDelete = false
                    viewModel.delete(onDeleted)
                }
            },
        )
    }
}

/** THE prompt form: the identity row (icon + name), then the markdown body. */
@Composable
private fun PromptForm(
    form: PromptFormState,
    editable: Boolean,
    creating: Boolean,
    bodyLoading: Boolean,
    bodyFailed: Boolean,
    onRetry: () -> Unit,
) {
    Column {
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
                            form.icon.takeIf { it.isNotEmpty() }?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault,
                            contentDescription = null,
                            modifier = Modifier.size(20.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                }
                Spacer(Modifier.width(12.dp))
                GlassTextField(
                    value = form.name,
                    onValueChange = { form.name = it.take(MAX_PROMPT_NAME) },
                    modifier = Modifier.weight(1f).testTag("prompt-edit-name"),
                    placeholder = "Name",
                    singleLine = true,
                    enabled = editable,
                )
            }
        }
        FieldError(form.nameError(creating), modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
        Spacer(Modifier.height(8.dp))
        OptionGroup {
            when {
                // EXP-1275: the fetch = skeleton rows, never "Loading…".
                bodyLoading -> SkeletonRows(dot = false, modifier = Modifier.padding(vertical = 4.dp))
                // FEED-79: the fetch ran on open, so no toast: the empty
                // state + Retry (a failed Retry tap toasts).
                bodyFailed -> Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
                ) {
                    Text(
                        "Prompt unavailable",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    TextButton(onClick = onRetry) { Text(FeedbackStates.RETRY) }
                }
                else -> GlassTextField(
                    value = form.body,
                    onValueChange = { form.body = it },
                    modifier = Modifier.fillMaxWidth().testTag("prompt-edit-body"),
                    placeholder = "Prompt",
                    minLines = 8,
                    enabled = editable,
                    bordered = false,
                    textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                )
            }
        }
        FieldError(form.bodyError, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
    }
}
