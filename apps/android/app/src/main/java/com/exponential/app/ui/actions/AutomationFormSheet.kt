package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.ui.components.GlassPillButton
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SectionLabel
import com.exponential.app.ui.issue.StartCodingSheetViewModel
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The "New automation" form (EXP-583) — the mobile twin of the web dialog:
 * pick an action, a trigger (Schedule | On event with the shared filter
 * pickers), the machine it runs on, and optionally pin an agent/model/effort.
 * Owner-only; the caller renders it and the server re-checks everything.
 *
 * Only AUTOMATABLE actions are offered: a real team action (never a builtin)
 * whose inputs are all optional, because an automated run has nobody to fill a
 * required one — the same rule the server enforces on create.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AutomationFormSheet(
    actions: List<ActionDto>,
    devices: List<SteerDevice>,
    busy: Boolean,
    error: String?,
    onSubmit: (actionId: String, deviceId: String, trigger: AutomationTrigger, agent: String?, model: String?, effort: String?) -> Unit,
    onDismiss: () -> Unit,
    dataViewModel: StartCodingSheetViewModel = hiltViewModel(),
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val labelOptions by dataViewModel.labelOptions.collectAsStateWithLifecycle()
    val statusOptions by dataViewModel.statusOptions.collectAsStateWithLifecycle()

    val targets = remember(actions) { actions.filter { it.automatable } }
    var actionId by remember { mutableStateOf(targets.firstOrNull()?.id) }
    var draft by remember { mutableStateOf(automationDraftFor(null)) }

    // Settle the pickers as the synced rows land, without stomping a manual
    // pick (both latches check for an existing value first).
    LaunchedEffect(targets) {
        if (actionId == null) actionId = targets.firstOrNull()?.id
    }
    LaunchedEffect(devices) {
        if (draft.deviceId == null) {
            devices.firstOrNull()?.let { draft = draft.copy(deviceId = it.deviceId) }
        }
    }

    val trigger = automationDraftToTrigger(draft)
    val deviceId = draft.deviceId
    val canCreate = actionId != null && deviceId != null && trigger != null && !busy

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = null,
        modifier = Modifier.statusBarsPadding().testTag("automation-form-sheet"),
    ) {
        Column(modifier = Modifier.fillMaxWidth().fillMaxHeight()) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                GlassPillButton(label = "Cancel", onClick = onDismiss)
                Spacer(Modifier.weight(1f))
                Text("New automation", style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.weight(1f))
                Button(
                    onClick = {
                        val action = actionId ?: return@Button
                        val device = deviceId ?: return@Button
                        val picked = trigger ?: return@Button
                        onSubmit(
                            action,
                            device,
                            picked,
                            draft.agent.takeIf { it.isNotEmpty() },
                            draft.model.takeIf { it.isNotEmpty() },
                            draft.effort.takeIf { it.isNotEmpty() },
                        )
                    },
                    enabled = canCreate,
                ) {
                    if (busy) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(14.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                    } else {
                        Text("Create")
                    }
                }
            }

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .verticalScroll(rememberScrollState()),
            ) {
                SectionLabel("Action")
                if (targets.isEmpty()) {
                    OptionGroup {
                        Text(
                            "No action can be automated yet. Automations run actions whose " +
                                "inputs are all optional.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                        )
                    }
                } else {
                    OptionGroup {
                        PickerRow(
                            label = "Runs",
                            value = targets.firstOrNull { it.id == actionId }?.name ?: "Select",
                            options = targets.map { it.id },
                            selected = actionId,
                            optionLabel = { id -> targets.firstOrNull { it.id == id }?.name ?: id },
                            onSelect = { actionId = it },
                        )
                    }
                }
                Spacer(Modifier.height(8.dp))

                SectionLabel("Trigger")
                AutomationTriggerFields(
                    draft = draft,
                    boards = boardOptions,
                    labels = labelOptions,
                    statuses = statusOptions,
                    onChange = { draft = it },
                )
                Spacer(Modifier.height(8.dp))

                SectionLabel("Machine")
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
}
