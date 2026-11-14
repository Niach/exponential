package com.exponential.app.domain

// #EXP-1832 (fixtures/sidebar-folders.json, its `$comment` IS the spec; the
// pure reference is `packages/domain-contract/src/sidebar-folders.ts`, the
// desktop's `domain::sidebar_folders`, iOS ExpCore `SidebarFolders.swift`).
// Each member arranges their own Boards, Agents and Apps into folders on the
// desktop IDE and wide web; the phone SHOWS that layout read-only (the board
// pickers, the Apps list): folders fold, nothing drags, nothing is created.
// `SidebarFoldersTest` replays every list of the fixture.

object SidebarFolders {
    const val SECTION_BOARD = "board"
    const val SECTION_AGENT = "agent"
    const val SECTION_APP = "app"
    const val KIND_FOLDER = "folder"
    const val KIND_ITEM = "item"
    const val DEFAULT_FOLDER_NAME = "New folder"
    const val FOLDER_NAME_MAX = 80

    /** Nesting levels: top level + one folder level. */
    const val MAX_DEPTH = 2

    /** One synced `sidebar_layout` row of a section. */
    data class Entry(
        val id: String,
        val kind: String,
        /** The board / agent / app an item places; null on a folder. */
        val targetId: String?,
        /** The folder an item sits in; null = top level. */
        val parentId: String?,
        /** A folder's name; null on an item. */
        val name: String?,
        val sortOrder: Double,
    )

    sealed interface Node {
        val id: String

        /** [id] = the target id (board / agent / app). */
        data class Item(override val id: String) : Node

        /** [id] = the folder entry's id; [children] = target ids, never empty. */
        data class Folder(override val id: String, val name: String, val children: List<String>) : Node
    }

    /** A List `move`: exactly one of [before] / [after] / [into]. */
    data class Move(val id: String, val before: String? = null, val after: String? = null, val into: String? = null)

    /** One row to write; [id] null = a new item entry (upserted by target). */
    data class EntryWrite(
        val id: String?,
        val kind: String,
        val targetId: String?,
        val parentId: String?,
        val name: String?,
        val sortOrder: Double,
    )

    data class Writes(val upserts: List<EntryWrite>, val deletes: List<String>, val createdFolderId: String?)

    /** One List row: a folder row is a `group` with its item count; a closed
     *  folder's items are left out. */
    data class Row(
        val kind: String,
        val id: String,
        val depth: Int,
        val group: Boolean,
        val name: String?,
        val open: Boolean,
        val count: Int,
    )

    private val entryOrder = compareBy<Entry>({ it.sortOrder }, { it.id })

    /** The section as the sidebar shows it ([items] = the visible targets in
     *  DEFAULT order: boards and agents by sortOrder, apps by name). */
    fun tree(items: List<String>, entries: List<Entry>): List<Node> {
        val visible = items.toSet()
        val folders = LinkedHashMap<String, Entry>()
        val itemEntries = LinkedHashMap<String, Entry>()
        for (entry in entries.sortedWith(entryOrder)) {
            if (entry.kind == KIND_FOLDER) {
                folders[entry.id] = entry
            } else if (entry.targetId != null && entry.targetId in visible && entry.targetId !in itemEntries) {
                itemEntries[entry.targetId] = entry
            }
        }
        val children = LinkedHashMap<String, MutableList<Entry>>()
        val top = mutableListOf<Entry>()
        for (entry in itemEntries.values) {
            if (entry.parentId != null && entry.parentId in folders) {
                children.getOrPut(entry.parentId) { mutableListOf() }.add(entry)
            } else {
                top.add(entry)
            }
        }
        for (folder in folders.values) if (folder.id in children) top.add(folder)
        val nodes = top.sortedWith(entryOrder).map { entry ->
            if (entry.kind == KIND_FOLDER) {
                Node.Folder(
                    id = entry.id,
                    name = entry.name ?: DEFAULT_FOLDER_NAME,
                    children = children.getValue(entry.id).sortedWith(entryOrder).map { it.targetId!! },
                )
            } else {
                Node.Item(entry.targetId!!)
            }
        }.toMutableList<Node>()
        for (item in items) if (item !in itemEntries) nodes.add(Node.Item(item))
        return nodes
    }

    private class Located(val folder: String?, val node: Node)

    private fun locate(tree: List<Node>, id: String): Located? {
        for (node in tree) {
            if (node.id == id) return Located(null, node)
            if (node is Node.Folder && id in node.children) return Located(node.id, Node.Item(id))
        }
        return null
    }

    private fun materialize(tree: List<Node>, entries: List<Entry>, deletes: List<String>, createdFolderId: String?): Writes {
        val byId = entries.associateBy { it.id }
        val byTarget = LinkedHashMap<String, Entry>()
        for (entry in entries.sortedWith(entryOrder)) {
            if (entry.kind == KIND_ITEM && entry.targetId != null && entry.targetId !in byTarget) byTarget[entry.targetId] = entry
        }
        val upserts = mutableListOf<EntryWrite>()
        fun writeItem(targetId: String, parentId: String?, sortOrder: Double) {
            val entry = byTarget[targetId]
            if (entry != null && entry.parentId == parentId && entry.sortOrder == sortOrder) return
            upserts.add(EntryWrite(entry?.id, KIND_ITEM, targetId, parentId, null, sortOrder))
        }
        tree.forEachIndexed { index, node ->
            val order = (index + 1).toDouble()
            when (node) {
                is Node.Item -> writeItem(node.id, null, order)
                is Node.Folder -> {
                    val entry = byId[node.id]
                    if (entry == null || entry.sortOrder != order || entry.name != node.name || entry.parentId != null) {
                        upserts.add(EntryWrite(node.id, KIND_FOLDER, null, null, node.name, order))
                    }
                    node.children.forEachIndexed { childIndex, child -> writeItem(child, node.id, (childIndex + 1).toDouble()) }
                }
            }
        }
        return Writes(upserts, deletes, createdFolderId)
    }

    /** Applies a List `move` to the section; null = refused (nothing written). */
    fun move(items: List<String>, entries: List<Entry>, move: Move, newFolderId: String): Writes? {
        val positions = listOfNotNull(move.before?.let { "before" to it }, move.after?.let { "after" to it }, move.into?.let { "into" to it })
        if (positions.size != 1) return null
        val (position, targetId) = positions.single()
        if (targetId == move.id) return null
        val tree = tree(items, entries).toMutableList()
        val dragged = locate(tree, move.id) ?: return null
        val target = locate(tree, targetId) ?: return null
        if (dragged.node is Node.Folder && (position == "into" || target.folder != null)) return null
        if (position == "into" && target.node is Node.Item && target.folder != null) return null

        fun folderAt(id: String) = tree.indexOfFirst { it.id == id }
        fun replaceChildren(folderId: String, transform: (MutableList<String>) -> Unit) {
            val index = folderAt(folderId)
            val folder = tree[index] as Node.Folder
            tree[index] = folder.copy(children = folder.children.toMutableList().also(transform))
        }

        // Take the dragged node out.
        val source = dragged.folder
        if (source != null) replaceChildren(source) { it.remove(move.id) } else tree.removeAt(folderAt(move.id))

        var createdFolderId: String? = null
        if (position == "into") {
            if (target.node is Node.Folder) {
                replaceChildren(targetId) { it.add(move.id) }
            } else {
                createdFolderId = newFolderId
                tree[folderAt(targetId)] = Node.Folder(newFolderId, DEFAULT_FOLDER_NAME, listOf(targetId, move.id))
            }
        } else {
            val offset = if (position == "after") 1 else 0
            if (target.folder != null) {
                replaceChildren(target.folder) { it.add(it.indexOf(targetId) + offset, move.id) }
            } else {
                tree.add(folderAt(targetId) + offset, dragged.node)
            }
        }

        val deletes = mutableListOf<String>()
        if (source != null) {
            val index = folderAt(source)
            if ((tree[index] as Node.Folder).children.isEmpty()) {
                tree.removeAt(index)
                deletes.add(source)
            }
        }
        return materialize(tree, entries, deletes, createdFolderId)
    }

    /** Ungroup: the folder's items take its place, the folder is deleted. */
    fun ungroup(items: List<String>, entries: List<Entry>, folderId: String): Writes? {
        val tree = tree(items, entries).toMutableList()
        val index = tree.indexOfFirst { it is Node.Folder && it.id == folderId }
        if (index < 0) return null
        val folder = tree.removeAt(index) as Node.Folder
        tree.addAll(index, folder.children.map { Node.Item(it) })
        return materialize(tree, entries, listOf(folderId), null)
    }

    /** A folder name as saved: trimmed, at most 80 characters, empty = the default. */
    fun folderName(input: String): String {
        val name = input.trim().take(FOLDER_NAME_MAX)
        return name.ifEmpty { DEFAULT_FOLDER_NAME }
    }

    /** The device-local expanded flag's key; no stored flag = OPEN. */
    fun openKey(teamId: String, folderId: String): String = "sidebar-folder-open:$teamId:$folderId"

    fun rows(tree: List<Node>, isOpen: (String) -> Boolean): List<Row> = buildList {
        for (node in tree) {
            when (node) {
                is Node.Item -> add(Row(KIND_ITEM, node.id, 0, group = false, name = null, open = false, count = 0))
                is Node.Folder -> {
                    val open = isOpen(node.id)
                    add(Row(KIND_FOLDER, node.id, 0, group = true, name = node.name, open = open, count = node.children.size))
                    if (open) node.children.forEach { add(Row(KIND_ITEM, it, 1, group = false, name = null, open = false, count = 0)) }
                }
            }
        }
    }
}
