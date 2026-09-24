package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the assignee picker: the team's members by avatar +
 * name, the email as the description and a search keyword; `Unassigned`
 * first. `ui/issue/AssigneePickerSheet` moves onto [Picker] in EXP-1021.
 */
data class AssigneePickerMember(
    val id: String,
    val name: String,
    val email: String? = null,
    val image: String? = null,
)

/** The row that clears the pick. */
const val UNASSIGNED_VALUE: String = ""

fun assigneePickerItems(
    members: List<AssigneePickerMember>,
    allowsNone: Boolean,
): List<PickerItem<String>> {
    val rows = members.map { member ->
        PickerItem(
            value = member.id,
            label = member.name,
            description = member.email,
            keywords = listOfNotNull(member.name, member.email).filter { it.isNotEmpty() },
        )
    }
    return if (allowsNone) listOf(PickerItem(value = UNASSIGNED_VALUE, label = "Unassigned")) + rows else rows
}

/**
 * [mode]: [PickerMode.Single] for the issue properties / create sheet (an
 * empty [value] = unassigned; [allowsNone] offers the `Unassigned` row, which
 * reports an EMPTY set), [PickerMode.Multi] for a filter ([allowsNone]
 * ignored).
 */
@Composable
fun AssigneePicker(
    members: List<AssigneePickerMember>,
    value: Set<String>,
    onChange: (Set<String>) -> Unit,
    mode: PickerMode = PickerMode.Single,
    allowsNone: Boolean = true,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    val single = mode == PickerMode.Single
    Picker(
        items = assigneePickerItems(members, allowsNone = single && allowsNone),
        mode = mode,
        value = if (single && value.isEmpty() && allowsNone) setOf(UNASSIGNED_VALUE) else value,
        onChange = { picked -> onChange(picked - UNASSIGNED_VALUE) },
        search = true,
        emptyText = "No members",
        title = "Assignee",
        trigger = trigger,
    )
}
