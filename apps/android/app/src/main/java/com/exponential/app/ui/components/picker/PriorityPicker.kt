package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.priorityIcon
import com.exponential.app.ui.theme.priorityColor

/**
 * EXP-1029 contract, EXP-1021 implementation — the priority picker: contract `issuePriority` in its
 * order, each by the priority glyph in its tone. The app hands in its
 * priority option rows; the picker never owns the table.
 */
data class PriorityPickerOption(
    /** Contract `issuePriority`. */
    val value: String,
    val label: String,
    val colorHex: String? = null,
)

fun priorityPickerItems(options: List<PriorityPickerOption>): List<PickerItem<String>> =
    options.map { option ->
        // Glyph and tone are FUNCTIONS of the contract value (`priorityIcon` /
        // `priorityColor`), so a caller can never hand in a priority row that
        // draws differently from the same priority in a list.
        val priority = IssuePriority.fromWire(option.value)
        PickerItem(
            value = option.value,
            label = option.label,
            icon = priorityIcon(priority),
            color = priorityColor(priority),
        )
    }

@Composable
fun PriorityPicker(
    options: List<PriorityPickerOption>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    mode: PickerMode = PickerMode.Single,
    /** The sheet headline; the default names the picker. */
    title: String = "Priority",
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
        items = priorityPickerItems(options),
        mode = mode,
        value = value,
        onChange = onChange,
        title = title,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
