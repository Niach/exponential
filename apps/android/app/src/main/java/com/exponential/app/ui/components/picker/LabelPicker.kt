package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import com.exponential.app.ui.parseColor

/**
 * EXP-1029 contract, EXP-1021 implementation — the label picker: ALWAYS multi, searchable, each row
 * its colour dot + name; the sheet stays open across toggles.
 * EXP-1021 moved `ui/issue/LabelPickerSheet` onto it, create row and all.
 */
data class LabelPickerLabel(
    val id: String,
    val name: String,
    /** The label's hex. */
    val colorHex: String? = null,
    /**
     * EXP-1021: the BULK edit's tri-state — [PickerChecked.Some] when only
     * part of the selection carries this label. Absent everywhere else, where
     * membership in `value` is the whole story.
     */
    val checked: PickerChecked? = null,
)

fun labelPickerItems(labels: List<LabelPickerLabel>): List<PickerItem<String>> =
    labels.map { label ->
        // No glyph: a colour on its own draws the primitive's DOT, which is
        // exactly how a label reads everywhere else in the app.
        PickerItem(
            value = label.id,
            label = label.name,
            color = label.colorHex?.takeIf { it.isNotBlank() }?.let(::parseColor),
            checked = label.checked,
        )
    }

@Composable
fun LabelPicker(
    labels: List<LabelPickerLabel>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    /** The sheet headline; the default names the picker. */
    title: String = "Labels",
    emptyText: String = "No labels",
    /** Typed under the rows — the "Create new label" row. */
    footer: (@Composable () -> Unit)? = null,
    query: String? = null,
    onQueryChange: ((String) -> Unit)? = null,
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
        items = labelPickerItems(labels),
        mode = PickerMode.Multi,
        value = value,
        onChange = onChange,
        search = true,
        emptyText = emptyText,
        title = title,
        footer = footer,
        query = query,
        onQueryChange = onQueryChange,
        searchPlaceholder = "Search labels",
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
