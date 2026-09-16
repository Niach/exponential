package com.exponential.app.domain

/**
 * EXP-916: the file TREE a Changes surface lists beside its file cards — the
 * sidebar on web (≥md) and the desktop, the file sheet on a phone. One pure
 * builder over the shared [Diff.File] model, hand-mirrored ×4 (TS
 * `domain-contract/src/diff-tree.ts`, web `@exp/ui` `FileDiffTree`, desktop
 * `domain::diff_tree`, iOS `ExpCore/Sources/Domain/DiffTree.swift`) and
 * byte-locked by `fixtures/diff/tree.json`.
 *
 * Rules:
 * - a path splits on `/`; every segment but the last is a directory;
 * - per level, directories come before files, each group sorted by the
 *   lower-cased name compared by UTF-16 code unit (never a locale compare — it
 *   is not portable), ties broken by the raw name the same way;
 * - a directory with exactly ONE child, a directory, and no files of its own
 *   compacts into that child — `apps/web/src` is one node (VS Code's compact
 *   folders), and the compaction repeats down the chain;
 * - a directory's additions/deletions/files are its subtree sums;
 * - a non-blank query returns a FLAT list of the FILE nodes whose path contains
 *   it, case-insensitively, in INPUT order — the filter is a search result, not
 *   a pruned tree.
 */
object DiffTree {

    enum class Kind(val wire: String) { DIR("dir"), FILE("file") }

    data class Node(
        val kind: Kind,
        /** The full path from the root (`apps/web/src` for a compacted dir). */
        val path: String,
        /** The label: the last segment, or the compacted chain `a/b/c`. */
        val name: String,
        val additions: Int,
        val deletions: Int,
        /** Files in the subtree (1 for a file). */
        val files: Int,
        /** The index into the input `files`; -1 for a directory. */
        val index: Int,
        val children: List<Node>,
    )

    private class Building(val path: String, val name: String) {
        val dirs = LinkedHashMap<String, Building>()
        val leaves = mutableListOf<Node>()
    }

    /** Lower-cased code-unit order, then the raw name — portable everywhere. */
    private val BY_NAME = Comparator<Node> { a, b ->
        val la = a.name.lowercase()
        val lb = b.name.lowercase()
        when {
            la < lb -> -1
            la > lb -> 1
            a.name < b.name -> -1
            a.name > b.name -> 1
            else -> 0
        }
    }

    private fun finish(dir: Building): Node {
        var dirs = dir.dirs.values.toList()
        var leaves: List<Node> = dir.leaves
        var path = dir.path
        var name = dir.name
        // Compact a lone child directory into this one, down the chain.
        while (dirs.size == 1 && leaves.isEmpty() && path.isNotEmpty()) {
            val only = dirs[0]
            path = only.path
            name = "$name/${only.name}"
            dirs = only.dirs.values.toList()
            leaves = only.leaves
        }
        val children = dirs.map { finish(it) }.sortedWith(BY_NAME) + leaves.sortedWith(BY_NAME)
        var additions = 0
        var deletions = 0
        var files = 0
        for (child in children) {
            additions += child.additions
            deletions += child.deletions
            files += child.files
        }
        return Node(Kind.DIR, path, name, additions, deletions, files, -1, children)
    }

    private fun leaf(file: Diff.File, index: Int, name: String): Node = Node(
        kind = Kind.FILE,
        path = file.path,
        name = name,
        additions = file.additions,
        deletions = file.deletions,
        files = 1,
        index = index,
        children = emptyList(),
    )

    fun diffFileTree(files: List<Diff.File>, query: String = ""): List<Node> {
        val needle = query.trim().lowercase()
        if (needle.isNotEmpty()) {
            val out = mutableListOf<Node>()
            files.forEachIndexed { index, file ->
                if (!file.path.lowercase().contains(needle)) return@forEachIndexed
                out.add(leaf(file, index, file.path))
            }
            return out
        }
        val root = Building("", "")
        files.forEachIndexed { index, file ->
            val segments = file.path.split("/")
            var at = root
            for (i in 0 until segments.size - 1) {
                val name = segments[i]
                val path = segments.subList(0, i + 1).joinToString("/")
                at = at.dirs.getOrPut(name) { Building(path, name) }
            }
            at.leaves.add(leaf(file, index, segments[segments.size - 1]))
        }
        return finish(root).children
    }

    /**
     * The byte-lock projection: one line per node, two spaces per depth, a
     * directory as `name +a -d (files)` and a file as `name +a -d`. ASCII `-`,
     * like [Diff.render].
     */
    fun renderDiffTree(nodes: List<Node>): List<String> {
        val out = mutableListOf<String>()
        fun walk(list: List<Node>, depth: Int) {
            for (node in list) {
                val indent = "  ".repeat(depth)
                val counts = "+${node.additions} -${node.deletions}"
                out.add(
                    if (node.kind == Kind.DIR) {
                        "$indent${node.name} $counts (${node.files})"
                    } else {
                        "$indent${node.name} $counts"
                    },
                )
                walk(node.children, depth + 1)
            }
        }
        walk(nodes, 0)
        return out
    }
}
