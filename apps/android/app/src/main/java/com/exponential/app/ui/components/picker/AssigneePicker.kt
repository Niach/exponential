package com.exponential.app.ui.components.picker

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.MemberAvatar

/**
 * EXP-1029 contract, EXP-1021 implementation — the assignee picker: the team's members by avatar +
 * name, the email as the description and a search keyword; `Unassigned`
 * first. EXP-1021 retired `ui/issue/AssigneePickerSheet` for it.
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
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    val single = mode == PickerMode.Single
    val byId = remember(members) { members.associateBy { it.id } }
    Picker(
        items = assigneePickerItems(members, allowsNone = single && allowsNone),
        mode = mode,
        value = if (single && value.isEmpty() && allowsNone) setOf(UNASSIGNED_VALUE) else value,
        onChange = { picked -> onChange(picked - UNASSIGNED_VALUE) },
        search = true,
        emptyText = "No members",
        title = "Assignee",
        searchPlaceholder = "Search members",
        open = open,
        onOpenChange = onOpenChange,
        // The avatar is what makes a member row a MEMBER row, and it is not a
        // PickerItem slot (a picker glyph is an icon, never a photo) — so the
        // one row body that needs one draws it over the primitive's.
        // `Unassigned` keeps the plain body: there is nobody to picture.
        renderItem = { item ->
            val member = byId[item.value]
            if (member != null) {
                MemberAvatar(
                    imageUrl = member.image,
                    nameOrEmail = member.name,
                    userId = member.id,
                    size = 20.dp,
                )
                Spacer(Modifier.width(PickerDefaults.LeadingGap))
            }
            PickerItemBody(item)
        },
        trigger = trigger,
    )
}
