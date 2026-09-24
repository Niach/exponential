package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
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
            // same fallback `actionGlyph` (EXP-721's ONE resolver) draws, so a
            // row and the chip it becomes never wear different glyphs.
            icon = action.icon?.takeIf { it.isNotBlank() }?.let { ExpIcons.byName(it) }
                ?: ExpIcons.actionDefault,
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
     * What the sheet says with no row to offer — one text for "nothing to
     * pick" and for "nothing matched" (the primitive's rule). The composer
     * says what it is still waiting for there.
     */
    emptyText: String = "No actions",
    /** The SHEET's modifier — a caller's `testTag` (the composer's shot flow). */
    sheetModifier: Modifier = Modifier,
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
        emptyText = emptyText,
        title = title,
        sheetModifier = sheetModifier,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
