package com.exponential.app.domain

/**
 * #EXP-2124, #EXP-2109 (fixtures/sidebar-menus.json, its `$comment` IS the
 * spec; the pure reference is `packages/domain-contract/src/sidebar-menus.ts`):
 * THE one menu table of the sidebar's rows. A phone opens it on a long-press in
 * the Teams tab, minus [PHONE_DROPS]. `SidebarMenusTest` replays `menuCases`,
 * `newCases`, `phoneCases`, `moveCases` and compares the table.
 */
object SidebarMenus {
    const val SEPARATOR = "separator"

    val GROUPS: List<String> = listOf("open", "kind", "arrange", "settings", "danger")

    val COPY: Map<String, String> = linkedMapOf(
        "open" to "Open",
        "open-new-tab" to "Open in new tab",
        "new-chat" to "New chat",
        "new" to "New",
        "new-folder" to "New folder",
        "new-issue" to "New issue",
        "new-team" to "New team",
        "join-team" to "Join team",
        "add-device" to "Add device",
        "compact" to "Compact conversation",
        "new-conversation" to "New conversation",
        "edit" to "Edit",
        "rename" to "Rename",
        "copy-link" to "Copy link",
        "move" to "Move to folder",
        "ungroup" to "Ungroup",
        "add-to-sidebar" to "Add to sidebar",
        "remove-from-sidebar" to "Remove from sidebar",
        "settings" to "Settings",
        "archive" to "Archive",
        "delete" to "Delete",
        "top-level" to "Top level",
        "newTab" to "New tab",
        "linkCopied" to "Link copied",
    )

    /** Concept names. */
    val ICONS: Map<String, String> = linkedMapOf(
        "open" to "ui-arrow-right",
        "open-new-tab" to "ui-external-link",
        "new-chat" to "action-chat",
        "new" to "ui-add",
        "new-folder" to "ui-folder",
        "new-issue" to "nav-create-issue",
        "new-team" to "ui-add",
        "join-team" to "ui-invite",
        "add-device" to "ui-add",
        "compact" to "coding-compact",
        "new-conversation" to "action-chat",
        "edit" to "ui-edit",
        "rename" to "ui-edit",
        "copy-link" to "ui-link",
        "move" to "ui-folder",
        "ungroup" to "ui-minus",
        "add-to-sidebar" to "ui-add",
        "remove-from-sidebar" to "ui-minus",
        "settings" to "nav-settings",
        "archive" to "ui-archive",
        "delete" to "ui-delete",
    )

    /** One table entry, shown only while the [Input] flag [when] is true. */
    data class Entry(val id: String, val flag: String? = null)

    data class Input(val owner: Boolean = false, val mine: Boolean = false, val canDelete: Boolean = false) {
        fun flag(name: String): Boolean = when (name) {
            "owner" -> owner
            "mine" -> mine
            "canDelete" -> canDelete
            else -> false
        }
    }

    private fun kind(vararg groups: Pair<String, List<Entry>>): Map<String, List<Entry>> = mapOf(*groups)

    /** Kind → group → entries (an absent group is empty). */
    val KINDS: Map<String, Map<String, List<Entry>>> = linkedMapOf(
        "assistant" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "kind" to listOf(Entry("compact"), Entry("new-conversation")),
            "settings" to listOf(Entry("settings")),
        ),
        "home" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "kind" to listOf(Entry("new-chat")),
        ),
        "needs-you" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
        ),
        "chats" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "kind" to listOf(Entry("new-chat")),
        ),
        "drafts" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
        ),
        "devices" to kind(
            "kind" to listOf(Entry("add-device")),
        ),
        "device" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "arrange" to listOf(Entry("rename", "mine"), Entry("copy-link")),
            "settings" to listOf(Entry("settings")),
        ),
        "teams" to kind(
            "kind" to listOf(Entry("new-team"), Entry("join-team")),
        ),
        "team" to kind(
            "kind" to listOf(Entry("new"), Entry("new-folder")),
            "settings" to listOf(Entry("settings")),
        ),
        "folder" to kind(
            "arrange" to listOf(Entry("rename"), Entry("ungroup")),
        ),
        "board" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "kind" to listOf(Entry("new-issue")),
            "arrange" to listOf(Entry("copy-link"), Entry("move")),
            "settings" to listOf(Entry("settings", "owner")),
            "danger" to listOf(Entry("archive", "owner")),
        ),
        "app" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "kind" to listOf(Entry("edit")),
            "arrange" to listOf(Entry("copy-link"), Entry("move")),
            "settings" to listOf(Entry("settings")),
        ),
        "agent" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "kind" to listOf(Entry("compact"), Entry("new-conversation")),
            "arrange" to listOf(Entry("move")),
            "settings" to listOf(Entry("settings", "owner")),
        ),
        "session" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "arrange" to listOf(Entry("copy-link"), Entry("move"), Entry("remove-from-sidebar")),
        ),
        "active-session" to kind(
            "open" to listOf(Entry("open"), Entry("open-new-tab")),
            "arrange" to listOf(Entry("copy-link"), Entry("add-to-sidebar")),
            "danger" to listOf(Entry("delete", "canDelete")),
        ),
        "getting-started" to kind(
            "open" to listOf(Entry("open")),
        ),
    )

    val NEW_KINDS: List<String> = listOf("session", "agent", "app", "board")
    val NEW_OWNER_ONLY: List<String> = listOf("agent", "app", "board")
    val NEW_COPY: Map<String, String> = linkedMapOf(
        "session" to "Chat",
        "agent" to "Agent",
        "app" to "App",
        "board" to "Board",
    )
    val NEW_ICONS: Map<String, String> = linkedMapOf(
        "session" to "action-chat",
        "agent" to "settings-agents",
        "app" to "nav-apps",
        "board" to "nav-boards",
    )
    val PHONE_DROPS: List<String> = listOf("open-new-tab")

    /** A `danger` row: drawn destructive, asks first. */
    fun destructive(kind: String, action: String): Boolean =
        KINDS.getValue(kind)["danger"].orEmpty().any { it.id == action }

    /** A kind's menu: the groups in order, [SEPARATOR] between the non-empty ones. */
    fun itemMenu(kind: String, input: Input = Input()): List<String> = buildList {
        val table = KINDS.getValue(kind)
        for (group in GROUPS) {
            val rows = table[group].orEmpty().filter { it.flag == null || input.flag(it.flag) }
            if (rows.isEmpty()) continue
            if (isNotEmpty()) add(SEPARATOR)
            rows.forEach { add(it.id) }
        }
    }

    /** THE New list: a chat for every member, the rest owners'. */
    fun newKinds(owner: Boolean): List<String> = NEW_KINDS.filter { owner || it !in NEW_OWNER_ONLY }

    /** A row's long-press menu on a phone, in groups (no empty one). */
    fun phoneMenuGroups(kind: String, input: Input = Input()): List<List<String>> {
        val groups = mutableListOf(mutableListOf<String>())
        for (id in itemMenu(kind, input)) {
            if (id == SEPARATOR) groups.add(mutableListOf()) else if (id !in PHONE_DROPS) groups.last().add(id)
        }
        return groups.filter { it.isNotEmpty() }
    }

    /** `sidebarPhoneMenu`: [phoneMenuGroups] flattened with separators. */
    fun phoneMenu(kind: String, input: Input = Input()): List<String> =
        phoneMenuGroups(kind, input).flatMapIndexed { index, group -> if (index > 0) listOf(SEPARATOR) + group else group }

    data class MoveFolder(val id: String, val name: String?)

    /** [topLevel] = the folder holding it (Top level = `after` it); null = none. */
    data class MoveTargets(val folders: List<MoveFolder>, val topLevel: String?)

    data class MoveRow(val kind: String, val id: String, val depth: Int, val name: String?)

    /** Move to folder › over a team's rows, depth 0 = its top level. */
    fun moveTargets(rows: List<MoveRow>, id: String): MoveTargets {
        var topLevel: String? = null
        var holder: String? = null
        for (row in rows) {
            val folder = row.kind == SidebarFolders.KIND_FOLDER
            if (folder) holder = row.id else if (row.depth == 0) holder = null
            if (row.id == id) topLevel = if (folder) null else holder
        }
        return MoveTargets(
            folders = rows.filter { it.kind == SidebarFolders.KIND_FOLDER && it.id != topLevel }.map { MoveFolder(it.id, it.name) },
            topLevel = topLevel,
        )
    }
}
