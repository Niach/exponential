package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction

/**
 * The "New trigger" / "Edit trigger" form of an AGENT's Triggers (EXP-583;
 * EXP-1291: the trigger carries only its WHEN — the agent itself is the
 * runner): Schedule (THE schedule picker) | On event (a name). Owner-only; the
 * caller renders it and writes the agent's WHOLE trigger array
 * (`agents.update({triggers})`), a refusal toasts with the sheet open.
 * [onSubmit] hands back [editing]'s id (null = a new trigger) + the when-part.
 */
@Composable
fun TriggerFormSheet(
    busy: Boolean,
    onSubmit: (AgentTrigger) -> Unit,
    onDismiss: () -> Unit,
    /** The trigger being edited; null = add a new one. */
    editing: AgentTrigger? = null,
    /** Non-null = a Delete under the form (asks first, `Prompts.DeleteTrigger`). */
    onDelete: (() -> Unit)? = null,
) {
    var confirmDelete by remember { mutableStateOf(false) }
    if (confirmDelete && onDelete != null) {
        PromptAlert(
            prompt = Prompts.DeleteTrigger.prompt(),
            onDismiss = { confirmDelete = false },
            handlers = Prompts.DeleteTrigger.prompt().actions
                .filter { it.role != Prompts.Role.Cancel }
                .associate { action ->
                    action.id to {
                        confirmDelete = false
                        onDelete()
                    }
                },
        )
    }
    var draft by remember { mutableStateOf(triggerDraftFor(editing?.whenPart)) }

    val whenPart = triggerDraftToWhen(draft)
    val canSubmit = whenPart != null && !busy

    GlassSheet(
        title = if (editing == null) "New trigger" else "Edit trigger",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("trigger-form-sheet"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(
            label = if (editing == null) "Add trigger" else "Save changes",
            enabled = canSubmit,
            loading = busy,
            onClick = submit@{
                val picked = whenPart ?: return@submit
                onSubmit(
                    AgentTrigger(
                        id = editing?.id,
                        enabled = editing?.enabled ?: true,
                        whenPart = picked,
                    ),
                )
            },
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            TriggerWhenFields(
                draft = draft,
                onChange = { draft = it },
            )
            if (onDelete != null) {
                Spacer(Modifier.height(16.dp))
                GlassPill(
                    "Delete trigger",
                    icon = ExpIcons.uiDelete,
                    onClick = { confirmDelete = true },
                    contentColor = DesignTokens.Semantic.Red,
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .fillMaxWidth()
                        .testTag("trigger-form-delete"),
                )
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}
