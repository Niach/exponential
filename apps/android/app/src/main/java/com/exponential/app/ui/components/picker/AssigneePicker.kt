package com.exponential.app.ui.components.picker

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AgentSettings
import com.exponential.app.ui.components.AgentAvatar
import com.exponential.app.ui.components.MemberAvatar
import com.exponential.app.ui.icons.ExpIcons

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

/**
 * #EXP-1383 (agent-settings.json `assignee`): a team AGENT as an assignee
 * row, listed straight after the members (no heading) by sortOrder then name
 * (`AgentSettings.assigneeOptions`); the row = THE AGENT MARK + the name,
 * which search matches. Its value is `agent:<id>` ([assigneeAgentValue]); a
 * member's stays the bare user id, so [assigneeFromPickerValue] splits a pick
 * into the two exclusive columns.
 */
data class AssigneePickerAgent(
    val id: String,
    val name: String,
    val icon: String?,
    val color: String?,
    val sortOrder: Double = 0.0,
)

fun assigneeAgentValue(agentId: String): String = "${AgentSettings.ASSIGNEE_AGENT_PREFIX}$agentId"

/** The single picker's value for an issue's two exclusive columns. */
fun assigneePickerValue(assigneeId: String?, assigneeAgentId: String?): Set<String> = when {
    !assigneeAgentId.isNullOrEmpty() -> setOf(assigneeAgentValue(assigneeAgentId))
    !assigneeId.isNullOrEmpty() -> setOf(assigneeId)
    else -> emptySet()
}

/** A pick back into the columns: an agent clears the person, a person (or
 *  `Unassigned`) clears the agent. */
fun assigneeFromPickerValue(value: String?): AgentSettings.Assignee = when {
    value.isNullOrEmpty() -> AgentSettings.Assignee.NONE
    value.startsWith(AgentSettings.ASSIGNEE_AGENT_PREFIX) ->
        AgentSettings.Assignee(null, value.removePrefix(AgentSettings.ASSIGNEE_AGENT_PREFIX))
    else -> AgentSettings.Assignee(value, null)
}

/** The row that clears the pick. */
const val UNASSIGNED_VALUE: String = ""

fun assigneePickerItems(
    members: List<AssigneePickerMember>,
    allowsNone: Boolean,
    agents: List<AssigneePickerAgent> = emptyList(),
): List<PickerItem<String>> {
    val memberRows = members.map { member ->
        PickerItem(
            value = member.id,
            label = member.name,
            description = member.email,
            keywords = listOfNotNull(member.name, member.email).filter { it.isNotEmpty() },
        )
    }
    val agentRows = AgentSettings.sortedAssigneeAgents(agents) {
        AgentSettings.AgentRowLike(it.id, it.name, it.icon, it.color, it.sortOrder)
    }.map { agent ->
        PickerItem(value = assigneeAgentValue(agent.id), label = agent.name, keywords = listOf(agent.name))
    }
    val rows = memberRows + agentRows
    // `Unassigned` carries its OWN glyph (iOS's `uiUnassigned`): every member
    // row below it leads with an avatar, so a bare label there would read as a
    // row with its picture still loading.
    val none = PickerItem(value = UNASSIGNED_VALUE, label = AgentSettings.ASSIGNEE_UNASSIGNED_LABEL, icon = ExpIcons.uiUnassigned)
    return if (allowsNone) listOf(none) + rows else rows
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
    /** #EXP-1383: the team's agents, listed after the members. */
    agents: List<AssigneePickerAgent> = emptyList(),
) {
    val single = mode == PickerMode.Single
    val byId = remember(members) { members.associateBy { it.id } }
    val agentByValue = remember(agents) { agents.associateBy { assigneeAgentValue(it.id) } }
    Picker(
        items = assigneePickerItems(members, allowsNone = single && allowsNone, agents = agents),
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
            val agent = agentByValue[item.value]
            if (agent != null) {
                AgentAvatar(icon = agent.icon, color = agent.color, size = 20.dp)
                Spacer(Modifier.width(PickerDefaults.LeadingGap))
            } else if (member != null) {
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
