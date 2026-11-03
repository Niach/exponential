package com.exponential.app.domain

// #EXP-1661: team skills on a phone (read + pick), the Kotlin mirror of
// `packages/domain-contract/src/skills.ts`, locked against
// `fixtures/skills.json` (`SkillsTest`, same case names). Phones never
// create or edit a skill (`permissions.phone`), so no form validation here.

/** One team skill as the list, the + menu and the agent page read it. */
data class SkillOption(
    val id: String,
    val name: String,
    val description: String = "",
    val enabledByDefault: Boolean = false,
    /** #VAPP-213: a built-in (shipped, read-only for everyone). */
    val builtin: Boolean = false,
)

object Skills {
    const val TITLE = "Skills"
    const val DEFAULT_PILL = "Default"
    /** #VAPP-213 `builtin.pill`: a built-in's list pill and picker hint. */
    const val BUILTIN_PILL = "Built-in"
    /** `builtin.names`: the shipped built-ins. */
    val BUILTIN_NAMES = listOf("exponential-apps", "exponential-motion", "exponential-ui")
    /** `builtin.editorDelivery`: the order an app editor session gets them. */
    val BUILTIN_EDITOR_DELIVERY = listOf("exponential-apps", "exponential-ui", "exponential-motion")
    const val EMPTY_TITLE = "No skills yet"
    const val PICKER_EMPTY = "No skills"
    const val AGENT_ROW_LABEL = "Skills"
    const val AGENT_ROW_NONE = "None"
    const val FIELD_NAME = "Name"
    const val FIELD_DESCRIPTION = "Description"
    const val FIELD_BODY = "Instructions"
    const val FIELD_DEFAULT = "Pre-select in new sessions"

    const val PAGE_TEST_ID = "settings-skills-page"
    const val LIST_TEST_ID = "settings-skills-list"
    const val ROW_TEST_ID = "settings-skills-row"
}

/** Settings › Skills and the + submenu order: by name, code-point order. */
fun <T> sortSkills(skills: List<T>, name: (T) -> String): List<T> = skills.sortedWith { a, b -> name(a).compareTo(name(b)) }

fun sortSkills(skills: List<SkillOption>): List<SkillOption> = sortSkills(skills) { it.name }

/** The launch pick's seed (the MCP servers' rule): the [saved] pick clamped
 *  to skills that still exist, else the enabled-by-default ones; list order. */
fun preselectSkillIds(skills: List<SkillOption>, saved: List<String>?): List<String> {
    val sorted = sortSkills(skills)
    if (saved != null) {
        val picked = saved.toHashSet()
        return sorted.filter { it.id in picked }.map { it.id }
    }
    return sorted.filter { it.enabledByDefault }.map { it.id }
}

/** The + menu row's value: the number picked, none at zero. */
fun skillsMenuValue(count: Int): String? = if (count <= 0) null else count.toString()

/** The agent page row's value: the picked names as the one picker summary
 *  (`None` / `a` / `a, b` / `a, b +2`, web `pickerSummary`). */
fun agentSkillsValue(names: List<String>): String = when {
    names.isEmpty() -> Skills.AGENT_ROW_NONE
    names.size <= 2 -> names.joinToString(", ")
    else -> "${names.take(2).joinToString(", ")} +${names.size - 2}"
}

/** The list row's pill (`pillCases`): Built-in for a built-in (never
 *  Default), Default while a team skill is pre-selected, else none. */
fun skillPill(builtin: Boolean, enabledByDefault: Boolean): String? = when {
    builtin -> Skills.BUILTIN_PILL
    enabledByDefault -> Skills.DEFAULT_PILL
    else -> null
}

/** Whether the skill page is editable (`editCases`): owners, never on a built-in. */
fun canEditSkill(builtin: Boolean, owner: Boolean): Boolean = owner && !builtin

/** A built-in's file path (`filePathCases`): relative, `/`-separated, no
 *  empty, `.` or `..` segment, no backslash, never SKILL.md itself. */
fun isSkillFilePath(path: String): Boolean {
    if (path.isEmpty() || path == "SKILL.md" || path.contains('\\') || path.contains('\u0000')) return false
    return path.split("/").all { it != "" && it != "." && it != ".." }
}
