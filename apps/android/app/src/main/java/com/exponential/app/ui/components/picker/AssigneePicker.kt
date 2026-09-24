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

@Composable
fun AssigneePicker(
    members: List<AssigneePickerMember>,
    value: String?,
    onChange: (String?) -> Unit,
    allowsNone: Boolean = true,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = assigneePickerItems(members, allowsNone),
        mode = PickerMode.Single,
        value = setOfNotNull(value ?: if (allowsNone) UNASSIGNED_VALUE else null),
        onChange = { picked ->
            val first = picked.firstOrNull() ?: return@Picker
            onChange(if (first == UNASSIGNED_VALUE) null else first)
        },
        search = true,
        emptyText = "No members",
        title = "Assignee",
        trigger = trigger,
    )
}
