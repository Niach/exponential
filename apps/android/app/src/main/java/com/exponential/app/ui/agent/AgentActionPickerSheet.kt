package com.exponential.app.ui.agent

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.data.api.ActionDto
import com.exponential.app.ui.components.picker.ActionPicker
import com.exponential.app.ui.components.picker.ActionPickerAction

/**
 * EXP-825 → SLOP-28 (EXP-1259 D5): the "+" menu's Run action › PAGE — the
 * shared [ActionPicker] drawn INLINE as THE search list, pushed inside the
 * composer's menu sheet: action rows = the action's icon + its NAME only (no
 * description), the builtins first, the hidden Chat row never (it is what
 * "no subject" means). A tap picks, chips the action and closes the menu;
 * picking an action while issues are chipped SWAPS them out.
 */
@Composable
internal fun AgentActionPickerPage(
    actions: List<ActionDto>?,
    error: String?,
    selectedId: String?,
    onPick: (String) -> Unit,
) {
    ActionPicker(
        actions = actions.orEmpty().map { ActionPickerAction(id = it.id, name = it.name, icon = it.icon) },
        value = selectedId,
        onChange = onPick,
        // With no row to offer the page says why: skeleton rows while the
        // list loads, the load's error, or that nothing matched.
        emptyText = if (actions == null) error.orEmpty() else "No matching actions",
        loading = actions == null && error == null,
        inline = true,
        modifier = Modifier.testTag("agent-composer-actions-picker"),
    )
}
