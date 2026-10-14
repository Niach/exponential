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
 * description), the team's saved actions only (EXP-1291: no builtins; no
 * subject = a plain message). A tap picks, chips the action and closes the menu;
 * picking an action while issues are chipped SWAPS them out.
 */
@Composable
internal fun AgentActionPickerPage(
    actions: List<ActionDto>?,
    error: String?,
    selectedId: String?,
    onPick: (String) -> Unit,
) {
    // FEED-79: the list loads on its own, so a failed load is quiet: the
    // empty state, never a toast.
    ActionPicker(
        actions = actions.orEmpty().map { ActionPickerAction(id = it.id, name = it.name, icon = it.icon) },
        value = selectedId,
        onChange = onPick,
        // With no row to offer: skeleton rows while the list loads, else
        // that nothing matched.
        emptyText = "No matching actions",
        loading = actions == null && error == null,
        inline = true,
        modifier = Modifier.testTag("agent-composer-actions-picker"),
    )
}
