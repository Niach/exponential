package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the priority picker: contract `issuePriority` in its
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
    options.map { option -> PickerItem(value = option.value, label = option.label) }

@Composable
fun PriorityPicker(
    options: List<PriorityPickerOption>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    mode: PickerMode = PickerMode.Single,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = priorityPickerItems(options),
        mode = mode,
        value = value,
        onChange = onChange,
        title = "Priority",
        trigger = trigger,
    )
}
