package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.exponential.app.domain.SkillOption
import com.exponential.app.domain.Skills
import com.exponential.app.domain.sortSkills
import com.exponential.app.ui.icons.ExpIcons

/**
 * #EXP-1661: the skills multiselect, McpServerPicker's twin (the + menu's
 * `Skills ›` and the agent page's Skills row): the team's skills sorted by
 * name, the `ui-docs` glyph + name, the description as caption, multi.
 */
fun skillPickerItems(skills: List<SkillOption>): List<PickerItem<String>> =
    sortSkills(skills).map { skill ->
        PickerItem(
            value = skill.id,
            label = skill.name,
            icon = ExpIcons.uiDocs,
            description = skill.description.takeIf { it.isNotBlank() },
        )
    }

@Composable
fun SkillPicker(
    skills: List<SkillOption>,
    value: List<String>,
    /** One toggled skill id (added or removed). */
    onToggle: (String) -> Unit,
    title: String = Skills.TITLE,
    sheetModifier: Modifier = Modifier,
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = skillPickerItems(skills),
        mode = PickerMode.Multi,
        checkMarks = true,
        value = value.toSet(),
        onChange = { next ->
            val added = next.firstOrNull { it !in value }
            val removed = value.firstOrNull { it !in next }
            (added ?: removed)?.let(onToggle)
        },
        search = skills.size > 6,
        emptyText = Skills.PICKER_EMPTY,
        title = title,
        sheetModifier = sheetModifier,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
