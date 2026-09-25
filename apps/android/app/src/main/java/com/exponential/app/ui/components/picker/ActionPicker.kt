package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-1029 contract, EXP-1021 implementation — the action picker: the team's actions (and the two
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
        PickerItem(
            value = action.id,
            label = action.name,
            // The curated action glyph, else the generic action mark — the
            // same fallback the actions list draws.
            icon = action.icon?.takeIf { it.isNotBlank() }?.let { ExpIcons.byName(it) }
                ?: ExpIcons.navActions,
            description = action.description,
        )
    }

@Composable
fun ActionPicker(
    actions: List<ActionPickerAction>,
    value: String?,
    onChange: (String) -> Unit,
    /** The sheet headline; the default names the picker. */
    title: String = "Action",
    /**
     * EXP-1021: the sheet CONTROLLED by the caller, for a picker that is a
     * state machine rather than a chip (the issue screens open theirs from a
     * properties sheet that has already closed).
     */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = actionPickerItems(actions),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        search = true,
        emptyText = "No actions",
        title = title,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
