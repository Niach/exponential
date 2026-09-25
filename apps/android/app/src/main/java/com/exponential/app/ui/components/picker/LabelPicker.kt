package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the label picker: ALWAYS multi, searchable, each row
 * its colour dot + name; the sheet stays open across toggles.
 * `ui/issue/LabelPickerSheet` moves onto [Picker] in EXP-1021.
 */
data class LabelPickerLabel(
    val id: String,
    val name: String,
    /** The label's hex. */
    val colorHex: String? = null,
)

fun labelPickerItems(labels: List<LabelPickerLabel>): List<PickerItem<String>> =
    labels.map { label -> PickerItem(value = label.id, label = label.name) }

@Composable
fun LabelPicker(
    labels: List<LabelPickerLabel>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = labelPickerItems(labels),
        mode = PickerMode.Multi,
        value = value,
        onChange = onChange,
        search = true,
        emptyText = "No labels",
        title = "Labels",
        trigger = trigger,
    )
}
