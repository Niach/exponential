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
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.ui.agent.AgentLaunchDataViewModel
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction

/**
 * What the trigger form hands back on submit: the when-part, the machine it
 * runs on and the pins. A null pin means the bound machine's own launch
 * default; [account] is set only beside its [agent] (EXP-995).
 */
data class TriggerFormResult(
    val whenPart: AutomationTrigger,
    val deviceId: String,
    val agent: String?,
    val account: String?,
    val model: String?,
    val effort: String?,
)

/**
 * The "New trigger" / "Edit trigger" form (EXP-583; SLOP-2: a trigger lives on
 * its action, so the form lost its action picker) — the mobile twin of the web
 * dialog: the when-part (Schedule | On event with the shared filter pickers),
 * the machine it runs on, and optionally a pinned account/model/effort.
 * Owner-only; the caller renders it and the server re-checks everything — its
 * refusal (an action with required inputs, an incapable device) shows as
 * [error] with the sheet still open.
 */
@Composable
fun TriggerFormSheet(
    devices: List<SteerDevice>,
    busy: Boolean,
    error: String?,
    onSubmit: (TriggerFormResult) -> Unit,
    onDismiss: () -> Unit,
    /** The trigger being edited; null = add a new one. */
    editing: ActionTrigger? = null,
    dataViewModel: AgentLaunchDataViewModel = hiltViewModel(),
    // EXP-1021: the label/status filter rows carry a colour and a resolved
    // glyph, which the launcher's id+name input options do not.
    filterViewModel: TriggerFilterOptionsViewModel = hiltViewModel(),
) {
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val labelOptions by filterViewModel.labels.collectAsStateWithLifecycle()
    val statusOptions by filterViewModel.statuses.collectAsStateWithLifecycle()

    var draft by remember {
        mutableStateOf(
            if (editing == null) {
                triggerDraftFor(null)
            } else {
                triggerDraftFor(editing.whenPart, deviceId = editing.deviceId).copy(
                    agent = editing.agent.orEmpty(),
                    account = editing.account.orEmpty(),
                    model = editing.model.orEmpty(),
                    effort = editing.effort.orEmpty(),
                )
            },
        )
    }

    // Settle the machine as the synced rows land, without stomping a pick.
    LaunchedEffect(devices) {
        if (draft.deviceId == null) {
            // EXP-622: the caller's default machine, else the first candidate.
            (devices.firstOrNull { it.isDefault } ?: devices.firstOrNull())
                ?.let { draft = draft.copy(deviceId = it.deviceId) }
        }
    }

    val whenPart = triggerDraftToWhen(draft)
    val deviceId = draft.deviceId
    val canSubmit = deviceId != null && whenPart != null && !busy

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
                val device = deviceId ?: return@submit
                val picked = whenPart ?: return@submit
                onSubmit(
                    TriggerFormResult(
                        whenPart = picked,
                        deviceId = device,
                        agent = draft.agent.takeIf { it.isNotEmpty() },
                        // EXP-995: a profile is ONE agent's — it rides only
                        // beside its agent.
                        account = draft.account.takeIf {
                            it.isNotEmpty() && draft.agent.isNotEmpty()
                        },
                        model = draft.model.takeIf { it.isNotEmpty() },
                        effort = draft.effort.takeIf { it.isNotEmpty() },
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
            // EXP-698: no "Trigger" heading — the card's own kind strip says
            // what it is, and the heading only pushed the groups apart.
            TriggerWhenFields(
                draft = draft,
                boards = boardOptions,
                labels = labelOptions,
                statuses = statusOptions,
                onChange = { draft = it },
            )
            Spacer(Modifier.height(8.dp))

            TriggerBindingFields(
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

/** The one sentence a locked trigger row shows — the same wording on every
 * client, wherever a required input blocks enabling a trigger. */
internal const val REQUIRED_INPUTS_HINT =
    "This action has required inputs, and a triggered run has none to fill them with. " +
        "Make the inputs optional to enable it."
