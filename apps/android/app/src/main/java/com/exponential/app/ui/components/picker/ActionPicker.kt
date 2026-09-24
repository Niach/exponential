package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the action picker: the team's actions (and the two
 * listed builtins) by curated icon (`ActionGlyph`) + name. The composer's
 * action chip and the automation editor pick one.
 */
data class ActionPickerAction(
    val id: String,
    val name: String,
    /** Contract `boardIcon` (the curated action set); null = the default. */
    val icon: String? = null,
    val description: String? = null,
)

fun actionPickerItems(actions: List<ActionPickerAction>): List<PickerItem<String>> =
    actions.map { action ->
        PickerItem(value = action.id, label = action.name, description = action.description)
    }

@Composable
fun ActionPicker(
    actions: List<ActionPickerAction>,
    value: String?,
    onChange: (String) -> Unit,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = actionPickerItems(actions),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        search = true,
        emptyText = "No actions",
        title = "Action",
        trigger = trigger,
    )
}
