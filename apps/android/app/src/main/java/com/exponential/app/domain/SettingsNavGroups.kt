package com.exponential.app.domain

/**
 * S-3 (owner 2026-10-28): THE settings nav ×4 = `settings-nav.json` `nav`
 * (groups, injected `@lists`, `children`, `pages`, per-platform pages), the
 * Android mirror of TS `settingsNavGroups` (`navCases`, `SettingsNavTest`).
 * The Features and Advanced groups are gone; Voice lives on the Assistant
 * page.
 */
object SettingsNavGroups {
    const val PLATFORM = "android"

    const val ASSISTANT = "@assistant"
    const val AGENTS = "@agents"
    const val NEW_AGENT = "@new-agent"
    const val BOARDS = "@boards"
    const val NEW_BOARD = "@new-board"
    const val APPS = "@apps"
    const val NEW_APP = "@new-app"

    data class Page(val label: String, val icon: String, val visible: String, val platforms: List<String>? = null)

    data class Group(val id: String, val label: String, val platforms: List<String>? = null, val items: List<String>)

    val GROUPS: List<Group> = listOf(
        Group("team", "Team", items = listOf("general", "members", "issues", "repositories", "prompts", "widget", "storage", "billing", "import")),
        Group("boards", "Boards", items = listOf(BOARDS, NEW_BOARD, "archived-boards")),
        Group("agents", "Agents", items = listOf(ASSISTANT, AGENTS, NEW_AGENT, "mcp")),
        Group("apps", "Apps", items = listOf(APPS, NEW_APP)),
        Group("device", "This device", platforms = listOf("desktop"), items = listOf("tools", "coding-clis", "worktrees", "sessions")),
        Group("personal", "Personal", items = listOf("account", "notifications", "security", "about")),
    )

    val CHILDREN: Map<String, List<String>> = mapOf("issues" to listOf("labels", "statuses"))

    val PAGES: Map<String, Page> = linkedMapOf(
        "storage" to Page("Storage", "settings-storage", "owner"),
        "import" to Page("Import", "settings-import", "owner", listOf("web")),
        "archived-boards" to Page("Archived boards", "ui-archive", "owner"),
        "tools" to Page("Tools", "settings-tools", "all", listOf("desktop")),
        "coding-clis" to Page("Coding CLIs", "settings-agents", "all", listOf("desktop")),
        "worktrees" to Page("Worktrees", "settings-local-repos", "all", listOf("desktop")),
        "sessions" to Page("Sessions", "settings-sessions", "all", listOf("desktop")),
        "account" to Page("Account", "settings-account", "all"),
        "notifications" to Page("Notifications", "settings-notifications", "all"),
        "security" to Page("Security", "settings-api", "all"),
        "about" to Page("About", "settings-about", "all", listOf("desktop", "ios", "android")),
    )

    val ROW_PLATFORMS: Map<String, List<String>> = mapOf(
        "billing" to listOf("web"),
        "devices" to listOf("ios", "android", "web-mobile"),
    )

    val NEW_ENTRIES: Map<String, String> = mapOf(NEW_BOARD to "owner", NEW_AGENT to "owner", NEW_APP to "owner")

    private fun visible(visible: String, isOwner: Boolean, cloud: Boolean): Boolean = when (visible) {
        "all" -> true
        "owner" -> isOwner
        "ownerCloud" -> isOwner && cloud
        else -> false
    }

    /** The label + icon of a page id (a `rows` row or a `pages` page). */
    fun page(id: String): Page? {
        SettingsNav.ROWS.firstOrNull { it.id == id }?.let { return Page(it.label, it.icon, it.visible, ROW_PLATFORMS[id]) }
        return PAGES[id]
    }

    /** One item (page id or `@list`) for a viewer on a platform. */
    fun itemVisible(item: String, platform: String, isOwner: Boolean, cloud: Boolean): Boolean {
        if (item.startsWith("@")) {
            val v = NEW_ENTRIES[item] ?: return true
            return visible(v, isOwner, cloud)
        }
        val page = page(item) ?: return false
        if (page.platforms != null && platform !in page.platforms) return false
        return visible(page.visible, isOwner, cloud)
    }

    /** The nav's groups for a viewer on a platform: group id → items in
     *  order (an empty group is dropped). */
    fun groups(platform: String = PLATFORM, isOwner: Boolean, cloud: Boolean = false): LinkedHashMap<String, List<String>> {
        val out = LinkedHashMap<String, List<String>>()
        for (group in GROUPS) {
            if (group.platforms != null && platform !in group.platforms) continue
            val items = group.items.filter { itemVisible(it, platform, isOwner, cloud) }
            if (items.isNotEmpty()) out[group.id] = items
        }
        return out
    }

    fun groupLabel(id: String): String = GROUPS.first { it.id == id }.label
}
