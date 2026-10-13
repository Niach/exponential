package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.ui.agent.AgentLaunchDataViewModel
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction

/**
 * The "New trigger" / "Edit trigger" form of an AGENT's Schedule (EXP-583;
 * EXP-1291: the trigger carries only its WHEN — the agent itself is the
 * runner): Schedule | On event with the shared filter pickers. Owner-only;
 * the caller renders it and writes the agent's WHOLE trigger array
 * (`sessions.updateAgent({triggers})`), a refusal toasts with the sheet open.
 * [onSubmit] hands back [editing]'s id (null = a new trigger) + the when-part.
 */
@Composable
fun TriggerFormSheet(
    busy: Boolean,
    onSubmit: (AgentTrigger) -> Unit,
    onDismiss: () -> Unit,
    /** The trigger being edited; null = add a new one. */
    editing: AgentTrigger? = null,
    dataViewModel: AgentLaunchDataViewModel = hiltViewModel(),
    // EXP-1021: the label/status filter rows carry a colour and a resolved
    // glyph, which the launcher's id+name input options do not.
    filterViewModel: TriggerFilterOptionsViewModel = hiltViewModel(),
) {
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val labelOptions by filterViewModel.labels.collectAsStateWithLifecycle()
    val statusOptions by filterViewModel.statuses.collectAsStateWithLifecycle()

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
                boards = boardOptions,
                labels = labelOptions,
                statuses = statusOptions,
                onChange = { draft = it },
            )
            Spacer(Modifier.height(24.dp))
        }
    }
}
