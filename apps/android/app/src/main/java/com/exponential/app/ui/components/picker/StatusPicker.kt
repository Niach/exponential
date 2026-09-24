package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import com.exponential.app.ui.parseColor

/**
 * EXP-1029 contract, EXP-1021 implementation — the status picker: the team's statuses (EXP-314) in
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
    /**
     * EXP-1021: the RESOLVED glyph name + tint (`ResolvedIssueStatus.iconName`
     * and `resolvedStatusColor`, the web twin's `icon`). A builtin row's stored
     * hex is a near-neutral seed value that reads wrong, and a started row's
     * glyph is its position's pie clock — neither is derivable from the
     * category alone, so the caller hands both in.
     */
    val iconName: String? = null,
    val color: Color? = null,
)

fun statusPickerItems(statuses: List<StatusPickerStatus>): List<PickerItem<String>> =
    statuses.map { status ->
        PickerItem(
            value = status.id,
            label = status.name,
            icon = status.iconName?.let { com.exponential.app.ui.icons.ExpIcons.byName(it) },
            color = status.color ?: status.colorHex?.takeIf { it.isNotBlank() }?.let(::parseColor),
            keywords = listOf(status.name, status.category),
        )
    }

@Composable
fun StatusPicker(
    statuses: List<StatusPickerStatus>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    mode: PickerMode = PickerMode.Single,
    /** The sheet headline; the default names the picker. */
    title: String = "Status",
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
        items = statusPickerItems(statuses),
        mode = mode,
        value = value,
        onChange = onChange,
        emptyText = "No statuses",
        title = title,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
