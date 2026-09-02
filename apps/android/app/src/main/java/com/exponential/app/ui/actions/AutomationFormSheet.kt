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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.issue.StartCodingSheetViewModel
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The "New automation" / "Edit automation" form (EXP-583, editing since
 * EXP-615) — the mobile twin of the web dialog: pick an action, a trigger
 * (Schedule | On event with the shared filter pickers), the machine it runs
 * on, and optionally pin an agent/model/effort. Owner-only; the caller renders
 * it and the server re-checks everything.
 *
 * Every custom action is offered (builtins have no team row to target), but an
 * action with a REQUIRED input cannot be automated — an automated run has
 * nobody to fill it in — so picking one blocks the submit and explains why,
 * exactly like the web dialog's disabled rows.
 */
@Composable
fun AutomationFormSheet(
    actions: List<ActionDto>,
    devices: List<SteerDevice>,
    busy: Boolean,
    error: String?,
    onSubmit: (actionId: String, deviceId: String, trigger: AutomationTrigger, agent: String?, model: String?, effort: String?) -> Unit,
    onDismiss: () -> Unit,
    /** The row being edited; null = create a new automation. */
    editing: AutomationEntity? = null,
    dataViewModel: StartCodingSheetViewModel = hiltViewModel(),
) {
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val labelOptions by dataViewModel.labelOptions.collectAsStateWithLifecycle()
    val statusOptions by dataViewModel.statusOptions.collectAsStateWithLifecycle()

    // Custom actions only — builtins are server-shipped prompts with required
    // inputs and no team row to target (web parity).
    val targets = remember(actions) { actions.filter { !it.isBuiltin } }
    var actionId by remember {
        mutableStateOf(editing?.actionId ?: targets.firstOrNull { it.automatable }?.id)
    }
    var draft by remember {
        mutableStateOf(
            if (editing == null) {
                automationDraftFor(null)
            } else {
                automationDraftFor(
                    AutomationTrigger.parse(editing.trigger),
                    deviceId = editing.deviceId,
                ).copy(
                    agent = editing.agent.orEmpty(),
                    model = editing.model.orEmpty(),
                    effort = editing.effort.orEmpty(),
                )
            },
        )
    }

    // Settle the pickers as the synced rows land, without stomping a manual
    // pick (both latches check for an existing value first).
    LaunchedEffect(targets) {
        if (actionId == null) actionId = targets.firstOrNull { it.automatable }?.id
    }
    LaunchedEffect(devices) {
        if (draft.deviceId == null) {
            // EXP-622: the caller's default machine, else the first candidate.
            (devices.firstOrNull { it.isDefault } ?: devices.firstOrNull())
                ?.let { draft = draft.copy(deviceId = it.deviceId) }
        }
    }

    val trigger = automationDraftToTrigger(draft)
    val deviceId = draft.deviceId
    val selectedAction = targets.firstOrNull { it.id == actionId }
    val blockedByInputs = selectedAction != null && !selectedAction.automatable
    val canSubmit = actionId != null && deviceId != null && trigger != null &&
        !blockedByInputs && !busy

    GlassSheet(
        title = if (editing == null) "New automation" else "Edit automation",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("automation-form-sheet"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(
            // Web-parity wording (EXP-615).
            label = if (editing == null) "Create automation" else "Save changes",
            enabled = canSubmit,
            loading = busy,
            onClick = submit@{
                val action = actionId ?: return@submit
                val device = deviceId ?: return@submit
                val picked = trigger ?: return@submit
                onSubmit(
                    action,
                    device,
                    picked,
                    draft.agent.takeIf { it.isNotEmpty() },
                    draft.model.takeIf { it.isNotEmpty() },
                    draft.effort.takeIf { it.isNotEmpty() },
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
            // No "Action" section header — the row below already says it
            // (EXP-615 dedupe).
            if (targets.isEmpty()) {
                OptionGroup {
                    Text(
                        "No custom actions yet. Create one first, then automate it.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                }
            } else {
                OptionGroup {
                    PickerRow(
                        label = "Action",
                        value = selectedAction?.name ?: "Select",
                        options = targets.map { it.id },
                        selected = actionId,
                        optionLabel = { id -> targets.firstOrNull { it.id == id }?.name ?: id },
                        onSelect = { actionId = it },
                    )
                }
                if (blockedByInputs) {
                    Spacer(Modifier.height(4.dp))
                    Text(
                        REQUIRED_INPUTS_HINT,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp),
                    )
                }
            }
            Spacer(Modifier.height(8.dp))

            SectionHeader("Trigger", modifier = Modifier.padding(horizontal = 16.dp))
            AutomationTriggerFields(
                draft = draft,
                boards = boardOptions,
                labels = labelOptions,
                statuses = statusOptions,
                onChange = { draft = it },
            )
            Spacer(Modifier.height(8.dp))

            AutomationBindingFields(
                draft = draft,
                devices = devices,
                onChange = { draft = it },
            )

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

/** Same sentence the web dialog and the Automations tab show on a locked row —
 * one reason, one wording, wherever a required input blocks automating. */
internal const val REQUIRED_INPUTS_HINT =
    "This action has required inputs, and an automated run has none to fill them with. " +
        "Make the inputs optional to enable it."
