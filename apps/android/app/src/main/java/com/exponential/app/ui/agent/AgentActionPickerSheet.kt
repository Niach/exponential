package com.exponential.app.ui.agent

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.data.api.ActionDto
import com.exponential.app.ui.components.picker.ActionPicker
import com.exponential.app.ui.components.picker.ActionPickerAction

/**
 * EXP-825: the composer's ▶ tool — a SINGLE-select action list (the
 * Start-coding sheet's Actions tab, EXP-257): "Fix merge conflicts" first,
 * then "Create action" (listed here now that the create sheet is gone — the
 * creator run is just another action to pick), then the team's rows in server
 * order; the hidden Chat row never (it is what "no subject" means). A tap
 * picks and closes; picking an action while issues are chipped SWAPS them out.
 *
 * EXP-1030: the sheet IS the shared [ActionPicker] — the curated glyph, the
 * name and the description, searched and marked the way every other pick on
 * this phone is. The loading and error states ride the primitive's ONE empty
 * text: with no row to offer, what the sheet has to say is why.
 */
@Composable
internal fun AgentActionPickerSheet(
    actions: List<ActionDto>?,
    error: String?,
    selectedId: String?,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    ActionPicker(
        actions = actions.orEmpty().map { action ->
            ActionPickerAction(
                id = action.id,
                name = action.name,
                icon = action.icon,
                description = action.description?.takeIf { it.isNotBlank() },
            )
        },
        value = selectedId,
        // A single pick closes the primitive's sheet, which is what dismisses
        // this one (see `onOpenChange`) — the pick itself only chips it.
        onChange = onPick,
        title = "Actions",
        emptyText = when {
            actions == null && error != null -> error
            actions == null -> "Loading actions…"
            else -> "No matching actions."
        },
        sheetModifier = Modifier.testTag("agent-composer-actions-picker"),
        open = true,
        onOpenChange = { open -> if (!open) onDismiss() },
    )
}
