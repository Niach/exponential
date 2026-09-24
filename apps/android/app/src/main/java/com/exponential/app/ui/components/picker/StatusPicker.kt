package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the status picker: the team's statuses (EXP-314) in
 * display order, each by its glyph in its colour (the status-icons rule).
 * The issue header, the create-issue sheet and the bulk edit pick one.
 */
data class StatusPickerStatus(
    val id: String,
    val name: String,
    /** Contract `issueStatusCategory`. */
    val category: String,
    /** The resolved row colour (custom rows' hex, builtins' token). */
    val colorHex: String? = null,
)

fun statusPickerItems(statuses: List<StatusPickerStatus>): List<PickerItem<String>> =
    statuses.map { status ->
        PickerItem(value = status.id, label = status.name, keywords = listOf(status.name, status.category))
    }

@Composable
fun StatusPicker(
    statuses: List<StatusPickerStatus>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    mode: PickerMode = PickerMode.Single,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = statusPickerItems(statuses),
        mode = mode,
        value = value,
        onChange = onChange,
        emptyText = "No statuses",
        title = "Status",
        trigger = trigger,
    )
}
